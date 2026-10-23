//! Provider wire fixtures, consumed through the real SDK by units and service tests.

use afd_wire::inference::InferenceApi;
use bytes::Bytes;
use futures_util::stream;
use serde_json::{Value, json};

pub(crate) fn body(
    status: u16,
    bytes: Vec<u8>,
) -> (http::HeaderMap, rig_core::http_client::BoxedStream) {
    let mut headers = http::HeaderMap::new();
    let wire = if (200..300).contains(&status) {
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/event-stream"),
        );
        serde_json::from_slice(&bytes).map_or(bytes.clone(), |document| encode(&document))
    } else {
        bytes
    };
    (
        headers,
        Box::pin(stream::once(async move { Ok(Bytes::from(wire)) })),
    )
}

fn frame(document: &Value) -> String {
    format!("data: {document}\n\n")
}

fn encode(document: &Value) -> Vec<u8> {
    let api = if document.get("choices").is_some() {
        InferenceApi::Chat
    } else if document.get("candidates").is_some() {
        InferenceApi::Gemini
    } else if document.get("done").is_some() {
        InferenceApi::Ollama
    } else if document.get("stop_reason").is_some() {
        InferenceApi::Messages
    } else {
        InferenceApi::Responses
    };
    let text = match api {
        InferenceApi::Responses => frame(&json!({"type":"response.completed","response":document})),
        InferenceApi::Chat => {
            let choice = document.pointer("/choices/0").unwrap_or(&Value::Null);
            let first = frame(
                &json!({"id":"fixture","choices":[{"index":0,"delta":choice.get("message"),"finish_reason":null}]}),
            );
            let end = frame(
                &json!({"id":"fixture","choices":[{"index":0,"delta":{},"finish_reason":choice.get("finish_reason")}],"usage":document.get("usage")}),
            );
            format!("{first}{end}data: [DONE]\n\n")
        }
        InferenceApi::Messages => {
            let start = frame(
                &json!({"type":"message_start","message":{"id":"fixture","type":"message","role":"assistant","model":"fixture","content":[],"usage":{"input_tokens":document.pointer("/usage/input_tokens"),"output_tokens":0}}}),
            );
            let block = frame(
                &json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            );
            let delta = frame(
                &json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":document.pointer("/content/0/text")}}),
            );
            let stop = frame(&json!({"type":"content_block_stop","index":0}));
            let usage = frame(
                &json!({"type":"message_delta","delta":{"stop_reason":document.get("stop_reason")},"usage":{"output_tokens":document.pointer("/usage/output_tokens")}}),
            );
            let end = frame(&json!({"type":"message_stop"}));
            format!("{start}{block}{delta}{stop}{usage}{end}")
        }
        InferenceApi::Gemini => frame(document),
        InferenceApi::Ollama => format!("{document}\n"),
    };
    text.into_bytes()
}
