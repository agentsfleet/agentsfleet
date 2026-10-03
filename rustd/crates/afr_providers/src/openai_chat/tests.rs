#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afr_tools::catalog::WEB_SEARCH;
use serde_json::json;

use super::{Chat, DONE};
use crate::dialect::{Decode as _, Dialect as _};
use crate::provider::{Call, Chunk, Message, Usage};
use crate::test_support::{
    CALL_ID, FOLLOW_UP, INSTRUCTIONS, MODEL, OUTPUT, PREAMBLE, QUESTION, TOOL, body, calculator,
    conversation, decode, event, parameters, request,
};

#[test]
fn should_send_the_conversation_as_chat_messages_with_no_hosted_spec() {
    let messages = conversation();
    let parameters = parameters();
    let tools = [calculator(&parameters)];

    let sent = body(&Chat, &request(&messages, &tools, &[&WEB_SEARCH]));

    assert_eq!(sent["model"], MODEL);
    assert_eq!(sent["stream_options"], json!({"include_usage": true}));
    assert_eq!(
        sent["messages"],
        json!([
            {"role": "system", "content": INSTRUCTIONS},
            {"role": "user", "content": QUESTION},
            {"role": "assistant", "content": PREAMBLE, "tool_calls": [{"id": CALL_ID,
                "type": "function", "function": {"name": TOOL, "arguments": "{\"expression\":\"2+2\"}"}}]},
            {"role": "tool", "tool_call_id": CALL_ID, "content": OUTPUT},
            {"role": "user", "content": FOLLOW_UP}
        ])
    );
    assert_eq!(
        sent["tools"],
        json!([{"type": "function", "function": {"name": TOOL, "description": TOOL,
            "parameters": parameters}}]),
        "web_search has no chat spec; a call to it is the router's to refuse"
    );
}

#[test]
fn should_send_null_content_for_a_turn_that_only_called() {
    let messages = [Message::Assistant {
        text: String::new(),
        calls: Vec::new(),
    }];

    let sent = body(&Chat, &request(&messages, &[], &[]));

    assert_eq!(
        sent["messages"][1],
        json!({"role": "assistant", "content": null})
    );
    assert!(sent.get("tools").is_none());
}

#[test]
fn should_send_a_key_only_when_there_is_one() {
    let client = reqwest::Client::new();
    let keyed = Chat.authorize(client.post("http://localhost/"), "sk-key");
    let keyless = Chat.authorize(client.post("http://localhost/"), "");

    let keyed = keyed.build().unwrap();
    let keyless = keyless.build().unwrap();

    assert_eq!(
        keyed.headers()[reqwest::header::AUTHORIZATION],
        "Bearer sk-key"
    );
    assert!(
        keyless
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .is_none()
    );
}

#[test]
fn should_assemble_each_call_from_its_pieces_and_send_it_once_finished() {
    let mut decoder = Chat.decoder();

    let chunks = decode(
        &mut decoder,
        [
            json!({"choices": [{"delta": {"reasoning_content": "hm", "content": "Hi"}}]}),
            json!({"choices": [{"delta": {"tool_calls": [
                {"index": 0, "id": CALL_ID, "function": {"name": TOOL, "arguments": "{\"exp"}},
                {"index": 1, "id": "call-2", "function": {"name": TOOL, "arguments": ""}}]}}]}),
            json!({"choices": [{"delta": {"tool_calls": [
                {"index": 0, "function": {"arguments": "ression\":\"2+2\"}"}}]}}]}),
            json!({"choices": [{"delta": {}, "finish_reason": "tool_calls"}]}),
            json!({"choices": [], "usage": {"prompt_tokens": 30, "completion_tokens": 4,
                "prompt_tokens_details": {"cached_tokens": 10}}}),
        ],
    )
    .unwrap();
    let mut after = Vec::new();
    decoder
        .event(&event(DONE), &mut |chunk| after.push(chunk))
        .unwrap();

    assert!(decoder.ended());
    assert!(after.is_empty(), "every call went out once, at the finish");
    assert_eq!(
        chunks,
        [
            Chunk::reasoning("hm".to_owned()),
            Chunk::answer("Hi".to_owned()),
            Chunk::Call(Call {
                id: CALL_ID.to_owned(),
                name: TOOL.to_owned(),
                arguments: json!({"expression": "2+2"}),
            }),
            Chunk::Call(Call {
                id: "call-2".to_owned(),
                name: TOOL.to_owned(),
                arguments: json!({}),
            }),
            Chunk::Usage(Usage {
                input: 30,
                cached_input: 10,
                output: 4
            }),
        ]
    );
}

#[test]
fn should_keep_arguments_that_do_not_parse_as_the_text_the_model_wrote() {
    let chunks = decode(
        &mut Chat.decoder(),
        [
            json!({"choices": [{"delta": {"tool_calls": [
                {"index": 0, "id": CALL_ID, "function": {"name": TOOL, "arguments": "{not json"}}]}}]}),
            json!({"choices": [{"delta": {}, "finish_reason": "tool_calls"}]}),
        ],
    )
    .unwrap();

    assert_eq!(
        chunks,
        [Chunk::Call(Call {
            id: CALL_ID.to_owned(),
            name: TOOL.to_owned(),
            arguments: json!("{not json"),
        })]
    );
}

#[test]
fn should_not_end_a_turn_that_never_finished() {
    let mut decoder = Chat.decoder();

    decode(
        &mut decoder,
        [json!({"choices": [{"delta": {"content": "par"}}]})],
    )
    .unwrap();

    assert!(
        !decoder.ended(),
        "a stream closing here is a lost connection"
    );
}
