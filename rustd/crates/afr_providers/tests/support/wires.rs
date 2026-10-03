//! Each wire as the fake speaks it: a turn that calls a tool, a turn that
//! answers, where a turn posts, and where its next request carries a tool's
//! result.

use axum::http::HeaderMap;
use serde_json::{Value, json};

use super::Reply;

/// The usage every scripted turn reports.
const PROMPT_TOKENS: u64 = 10;
const COMPLETION_TOKENS: u64 = 3;

/// One of the three wires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wire {
    Messages,
    Responses,
    Chat,
}

impl Wire {
    /// Every wire, in the order the suites walk them.
    pub(crate) const ALL: [Self; 3] = [Self::Messages, Self::Responses, Self::Chat];

    /// The provider name that selects this wire at the fake.
    pub(crate) fn provider(self) -> String {
        match self {
            Self::Messages => "anthropic".to_owned(),
            Self::Responses => "openai".to_owned(),
            Self::Chat => super::CHAT_PROVIDER.to_owned(),
        }
    }

    /// Where a turn posts.
    pub(crate) const fn path(self) -> &'static str {
        match self {
            Self::Messages => "/v1/messages",
            Self::Responses => "/v1/responses",
            Self::Chat => "/v1/chat/completions",
        }
    }

    /// Whether `headers` carry `key` the way this wire sends it.
    pub(crate) fn carries_key(self, headers: &HeaderMap, key: &str) -> bool {
        let (name, value) = match self {
            Self::Messages => ("x-api-key", key.to_owned()),
            Self::Responses | Self::Chat => ("authorization", format!("Bearer {key}")),
        };
        headers.get(name).is_some_and(|sent| sent == value.as_str())
    }

    /// A turn that calls `name` with `arguments` under call id `id`.
    pub(crate) fn call(self, id: &str, name: &str, arguments: &Value) -> Reply {
        let raw = arguments.to_string();
        Reply::Stream(match self {
            Self::Messages => messages(vec![
                json!({"type": "content_block_start", "index": 0,
                    "content_block": {"type": "tool_use", "id": id, "name": name, "input": {}}}),
                json!({"type": "content_block_delta", "index": 0,
                    "delta": {"type": "input_json_delta", "partial_json": raw}}),
                json!({"type": "content_block_stop", "index": 0}),
            ]),
            Self::Responses => responses(vec![json!({"type": "response.output_item.done",
                "item": {"type": "function_call", "call_id": id, "name": name, "arguments": raw}})]),
            Self::Chat => chat(&json!({"tool_calls": [{"index": 0, "id": id,
                "function": {"name": name, "arguments": raw}}]})),
        })
    }

    /// A turn that answers `text`.
    pub(crate) fn answer(self, text: &str) -> Reply {
        Reply::Stream(match self {
            Self::Messages => messages(vec![json!({"type": "content_block_delta", "index": 0,
                "delta": {"type": "text_delta", "text": text}})]),
            Self::Responses => responses(vec![
                json!({"type": "response.output_text.delta", "delta": text}),
            ]),
            Self::Chat => chat(&json!({"content": text})),
        })
    }

    /// The tool results a request carries back to the model, in order.
    pub(crate) fn results(self, body: &Value) -> Vec<String> {
        let (list, kind_key, kind, output) = match self {
            Self::Messages => ("messages", "type", "tool_result", "content"),
            Self::Responses => ("input", "type", "function_call_output", "output"),
            Self::Chat => ("messages", "role", "tool", "content"),
        };
        let items = body[list].as_array().into_iter().flatten();
        let blocks = items.flat_map(|item| match &item["content"] {
            Value::Array(blocks) if self == Self::Messages => blocks.clone(),
            _ => vec![item.clone()],
        });
        blocks
            .filter(|block| block[kind_key] == kind)
            .filter_map(|block| block[output].as_str().map(str::to_owned))
            .collect()
    }

    /// The tool names a request offers, hosted specs by their type.
    pub(crate) fn offered(self, body: &Value) -> Vec<String> {
        let tools = body["tools"].as_array().into_iter().flatten();
        tools
            .filter_map(|tool| {
                let named = match self {
                    Self::Chat => &tool["function"]["name"],
                    Self::Messages | Self::Responses => &tool["name"],
                };
                named
                    .as_str()
                    .or_else(|| tool["type"].as_str())
                    .map(str::to_owned)
            })
            .collect()
    }
}

/// A Messages turn: `blocks` between its start and its end.
fn messages(blocks: Vec<Value>) -> Vec<String> {
    let start = json!({"type": "message_start",
        "message": {"usage": {"input_tokens": PROMPT_TOKENS}}});
    let delta = json!({"type": "message_delta", "usage": {"output_tokens": COMPLETION_TOKENS}});
    let events = std::iter::once(start)
        .chain(blocks)
        .chain([delta, json!({"type": "message_stop"})]);
    events.map(|data| framed(&data)).collect()
}

/// A Responses turn: `events`, then its completion.
fn responses(events: Vec<Value>) -> Vec<String> {
    let completed = json!({"type": "response.completed", "response": {"usage":
        {"input_tokens": PROMPT_TOKENS, "output_tokens": COMPLETION_TOKENS}}});
    events
        .into_iter()
        .chain([completed])
        .map(|data| framed(&data))
        .collect()
}

/// A chat turn: one `delta`, its finish, its usage, and the end line.
fn chat(delta: &Value) -> Vec<String> {
    let chunks = [
        json!({"choices": [{"index": 0, "delta": delta}]}),
        json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]}),
        json!({"choices": [], "usage":
            {"prompt_tokens": PROMPT_TOKENS, "completion_tokens": COMPLETION_TOKENS}}),
    ];
    let mut events: Vec<String> = chunks.iter().map(framed).collect();
    events.push("data: [DONE]\n\n".to_owned());
    events
}

/// One Server-Sent Event carrying `data`, named by its `type` when it has one,
/// as Messages and Responses name theirs.
fn framed(data: &Value) -> String {
    match data["type"].as_str() {
        Some(name) => format!("event: {name}\ndata: {data}\n\n"),
        None => format!("data: {data}\n\n"),
    }
}
