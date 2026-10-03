#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_wire::report::FailureClass;
use afr_tools::catalog::WEB_SEARCH;
use serde_json::json;

use super::Responses;
use crate::dialect::{Decode as _, Dialect as _};
use crate::provider::{Call, Chunk, Usage};
use crate::test_support::{
    CALL_ID, FOLLOW_UP, INSTRUCTIONS, MODEL, OUTPUT, PREAMBLE, QUESTION, TOOL, body, calculator,
    conversation, decode, parameters, request,
};

#[test]
fn should_send_the_conversation_as_input_items_and_keep_nothing_stored() {
    let messages = conversation();
    let parameters = parameters();
    let tools = [calculator(&parameters)];

    let sent = body(&Responses, &request(&messages, &tools, &[&WEB_SEARCH]));

    assert_eq!(sent["model"], MODEL);
    assert_eq!(sent["instructions"], INSTRUCTIONS);
    assert_eq!(sent["stream"], true);
    assert_eq!(sent["store"], false, "no run is kept at the provider");
    assert_eq!(
        sent["input"],
        json!([
            {"type": "message", "role": "user", "content": QUESTION},
            {"type": "message", "role": "assistant", "content": PREAMBLE},
            {"type": "function_call", "call_id": CALL_ID, "name": TOOL,
                "arguments": "{\"expression\":\"2+2\"}"},
            {"type": "function_call_output", "call_id": CALL_ID, "output": OUTPUT},
            {"type": "message", "role": "user", "content": FOLLOW_UP}
        ])
    );
    assert_eq!(
        sent["tools"],
        json!([
            {"type": "function", "name": TOOL, "description": TOOL, "parameters": parameters},
            {"type": WEB_SEARCH.name()}
        ])
    );
}

#[test]
fn should_carry_the_key_as_a_bearer_token() {
    let built = Responses
        .authorize(reqwest::Client::new().post("http://localhost/"), "sk-key")
        .build()
        .unwrap();

    assert_eq!(
        built.headers()[reqwest::header::AUTHORIZATION],
        "Bearer sk-key"
    );
}

#[test]
fn should_read_text_reasoning_a_finished_call_and_the_turns_usage() {
    let mut decoder = Responses.decoder();

    let chunks = decode(
        &mut decoder,
        [
            json!({"type": "response.created", "response": {}}),
            json!({"type": "response.reasoning_summary_text.delta", "delta": "hm"}),
            json!({"type": "response.output_text.delta", "delta": "Hi"}),
            json!({"type": "response.function_call_arguments.delta", "delta": "{\"exp"}),
            json!({"type": "response.output_item.done", "item": {"type": "function_call",
                "call_id": CALL_ID, "name": TOOL, "arguments": "{\"expression\":\"2+2\"}"}}),
            json!({"type": "response.output_item.done", "item": {"type": "message"}}),
            json!({"type": "response.completed", "response": {"usage": {"input_tokens": 20,
                "output_tokens": 5, "input_tokens_details": {"cached_tokens": 8}}}}),
        ],
    )
    .unwrap();

    assert!(decoder.ended());
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
            Chunk::Usage(Usage {
                input: 20,
                cached_input: 8,
                output: 5
            }),
        ]
    );
}

#[test]
fn should_end_an_incomplete_turn_with_what_it_has() {
    let mut decoder = Responses.decoder();

    decode(
        &mut decoder,
        [json!({"type": "response.incomplete", "response": {"usage": null}})],
    )
    .unwrap();

    assert!(decoder.ended());
}

#[test]
fn should_end_a_failed_turn_with_the_providers_code() {
    let failed = decode(
        &mut Responses.decoder(),
        [json!({"type": "response.failed", "response": {"error": {"code": "server_error", "message": "x"}}})],
    )
    .unwrap_err();
    let errored = decode(
        &mut Responses.decoder(),
        [json!({"type": "error", "code": null})],
    )
    .unwrap_err();

    assert!(
        failed.detail().ends_with("server_error"),
        "{}",
        failed.detail()
    );
    assert_eq!(failed.failure_class(), Some(FailureClass::TransportLoss));
    assert!(
        errored.detail().ends_with(super::UNNAMED),
        "{}",
        errored.detail()
    );
}
